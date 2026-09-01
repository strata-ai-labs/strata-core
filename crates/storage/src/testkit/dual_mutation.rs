//! TCP4.6c — the dual-mutation fuzz harness (dbsqlfuzz mold).
//!
//! One fuzz input co-mutates BOTH sides of the durability contract: the
//! operation stream (commits, deletes, flush, checkpoint) and the on-disk
//! bytes (any file under the store root — WAL segments, table blocks,
//! snapshots, manifests), interleaved across close → damage → reopen epochs.
//! `SQLite` reports this design (`dbsqlfuzz`) out-yields every one of its
//! other fuzzers because the damage lands on state the *continued* workload
//! then exercises — single-shot damage-then-verify never reaches those paths.
//!
//! Oracle stack, judged at every reopen:
//! - A reopen may REFUSE loudly (a typed open error) — safe, the case ends.
//! - A reopen that succeeds **`Healthy`** must be TRUTHFUL: the model is
//!   truncated to the runtime's own `recovered_visible_version` claim and
//!   classified under `CrashFamily::ZeroLoss` — the recovered state must be
//!   exactly the acknowledged history through the claimed watermark. A state
//!   below the claim (health-vs-truth), a hole, a torn batch, or a
//!   fabricated/resurrected row is a violation and panics the target.
//! - A reopen that succeeds **`Degraded`/`Failed`** has DECLARED loss: a
//!   lower watermark is legal, but the state must still be a valid prefix of
//!   acked history (fabrication, holes, and torn batches still violate), and
//!   the case ends — degraded health blocks mutating admission with no
//!   acknowledge path by design.
//! - A read that fails loudly after a damaged reopen is safe (fail-closed)
//!   and ends the case; only silent wrong data is a finding.
//!
//! Epochs end with a CLEAN close before damage: files are quiescent, the
//! listing is stable, and cases replay deterministically. The crash-timing
//! dimension is the recovery oracle's and the whole-DB simulation's job;
//! this target owns the damage-surface × continued-operation product.
//! Durability is `Standard` — the adopt-the-claim oracle is equally strict
//! under either policy and Standard roughly halves per-exec fsync cost.

use std::path::{Path, PathBuf};

use strata_core::{BranchId, CommitVersion};

use super::recovery_oracle::model::{ExpectedState, OracleDurability, RecordedMutation};
use super::recovery_oracle::verify::{classify_recovered, scan_recovered, CrashFamily};
use super::recovery_oracle::workload::{
    default_branch, oracle_key, oracle_prefix_key, oracle_space, to_commit_mutation, KEY_SPACE,
    SCAN_LIMIT,
};
use crate::api::{
    CommitBatch, CommitOptions, MaintenanceRequest, MaintenanceScope, MaintenanceTask,
    RecoveryHealthSummary, StorageDurabilityPolicy, StorageOpenOptions, StorageRuntime,
    StorageValue,
};
use crate::backend::{BackendError, BackendErrorKind};
use crate::testkit::TestkitError;

/// Epoch cap per input — enough interleaving depth to operate on a
/// damaged-then-recovered store repeatedly without unbounded per-exec cost.
const MAX_EPOCHS: usize = 4;
/// Ops per epoch draw is `count % (MAX_OPS_PER_EPOCH + 1)`.
const MAX_OPS_PER_EPOCH: u8 = 12;
/// Damage directives per epoch draw is `count % (MAX_DAMAGE_PER_EPOCH + 1)`;
/// zero-damage epochs are legal and give free clean-reopen coverage.
const MAX_DAMAGE_PER_EPOCH: u8 = 3;
/// Junk byte appended by the extend action.
const JUNK_BYTE: u8 = 0xA5;

/// Counted facts of one dual-mutation case — the non-vacuity record the unit
/// tests pin exactly, so every grammar arm is provably reachable.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DualMutationOutcome {
    /// Epochs whose reopen (or end-of-input close) completed.
    pub epochs_completed: usize,
    /// Single-put commits acknowledged.
    pub puts: usize,
    /// Two-mutation-batch commits acknowledged (torn-batch sensitive).
    pub paired_puts: usize,
    /// Delete commits acknowledged.
    pub deletes: usize,
    /// Flush maintenance rounds driven.
    pub flushes: usize,
    /// Checkpoint maintenance rounds driven.
    pub checkpoints: usize,
    /// Byte flips applied.
    pub damage_flips: usize,
    /// Truncations applied.
    pub damage_truncates: usize,
    /// Junk extensions applied.
    pub damage_appends: usize,
    /// Whole-file deletions applied.
    pub damage_deletes: usize,
    /// Damage directives skipped (no files, or a flip on an empty file).
    pub damage_skipped: usize,
    /// Reopens that succeeded `Healthy` and passed claim-strict classification.
    pub reopens_classified: usize,
    /// Reopens that declared loss (`Degraded`/`Failed`) onto a valid prefix
    /// of acked history — legal; the case ends.
    pub degraded_prefixes: usize,
    /// Reopens refused with a typed error — fail-closed, safe.
    pub loud_refusals: usize,
    /// Enqueue/drain scheduling races on the real threaded runtime — the
    /// background lane took the task first ("no longer startable"). Benign:
    /// the maintenance work runs either way.
    pub maintenance_races: usize,
    /// Maintenance failures matching the OPEN bug #3015 (manifest loss
    /// reopens Healthy; the reseeded snapshot-id allocator collides with an
    /// orphaned snapshot object — `AlreadyExists` on publish). Allowed and
    /// case-ending while #3015 is open; DELETE this allowance when the fix
    /// lands (the pin test flips first).
    pub pin_3015_snapshot_collisions: usize,
    /// Post-reopen scans refused with a typed error — fail-closed, safe.
    /// A fail-safe arm: recovery validates structural objects at open, so no
    /// deterministic constructor reaches it today (both probe shapes —
    /// manifest flip, table-object flip — refuse at OPEN instead).
    pub read_refusals: usize,
}

/// Deterministic byte cursor over the fuzz input — libFuzzer supplies the
/// entropy; the grammar never draws randomness of its own.
struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }

    fn next(&mut self) -> Option<u8> {
        let byte = self.data.get(self.at).copied();
        if byte.is_some() {
            self.at += 1;
        }
        byte
    }

    fn next_u16(&mut self) -> Option<u16> {
        let hi = self.next()?;
        let lo = self.next()?;
        Some(u16::from_be_bytes([hi, lo]))
    }
}

fn open_options() -> StorageOpenOptions {
    // Lossy recovery from the start: damage may be non-tail, and the oracle —
    // not strictness — is what forbids silent wrong data.
    StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard).with_strict_recovery(false)
}

fn open_store(root: &Path) -> Result<crate::api::StorageOpenOutcome<'static>, TestkitError> {
    // Retry-on-Unavailable absorbs any residual writer-lock window (#2727).
    crate::testkit::reopen_retry::open_with_retry_on_unavailable(|| {
        StorageRuntime::open_durable_local_with_options(root.to_path_buf(), open_options())
    })
    .map_err(|err| TestkitError::new(format!("initial open: {err:?}")))
}

/// Sorted recursive listing of every regular file under `root` — the
/// deterministic damage-target index space.
fn list_files(root: &Path) -> Result<Vec<PathBuf>, TestkitError> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .map_err(|err| TestkitError::new(format!("list {}: {err}", dir.display())))?;
        for entry in entries {
            let entry = entry.map_err(|err| TestkitError::new(format!("list entry: {err}")))?;
            let path = entry.path();
            let kind = entry
                .file_type()
                .map_err(|err| TestkitError::new(format!("file type: {err}")))?;
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Applies one damage directive to the closed store's files.
fn apply_damage(
    root: &Path,
    file_select: u8,
    action_select: u8,
    offset_select: u16,
    outcome: &mut DualMutationOutcome,
) -> Result<(), TestkitError> {
    let files = list_files(root)?;
    if files.is_empty() {
        outcome.damage_skipped += 1;
        return Ok(());
    }
    let path = &files[usize::from(file_select) % files.len()];
    let offset = u64::from(offset_select);
    match action_select % 4 {
        0 => {
            let mut bytes = std::fs::read(path)
                .map_err(|err| TestkitError::new(format!("damage read: {err}")))?;
            if bytes.is_empty() {
                outcome.damage_skipped += 1;
                return Ok(());
            }
            let index = usize::try_from(offset).unwrap_or(usize::MAX) % bytes.len();
            bytes[index] ^= 0xFF;
            std::fs::write(path, bytes)
                .map_err(|err| TestkitError::new(format!("damage write: {err}")))?;
            outcome.damage_flips += 1;
        }
        1 => {
            let len = std::fs::metadata(path)
                .map_err(|err| TestkitError::new(format!("damage metadata: {err}")))?
                .len();
            let file = std::fs::OpenOptions::new()
                .write(true)
                .open(path)
                .map_err(|err| TestkitError::new(format!("damage open: {err}")))?;
            file.set_len(offset % (len + 1))
                .map_err(|err| TestkitError::new(format!("damage truncate: {err}")))?;
            outcome.damage_truncates += 1;
        }
        2 => {
            let junk_len = 1 + usize::from(offset_select % 64);
            let mut bytes = std::fs::read(path)
                .map_err(|err| TestkitError::new(format!("damage read: {err}")))?;
            bytes.extend(std::iter::repeat_n(JUNK_BYTE, junk_len));
            std::fs::write(path, bytes)
                .map_err(|err| TestkitError::new(format!("damage write: {err}")))?;
            outcome.damage_appends += 1;
        }
        _ => {
            std::fs::remove_file(path)
                .map_err(|err| TestkitError::new(format!("damage delete: {err}")))?;
            outcome.damage_deletes += 1;
        }
    }
    Ok(())
}

/// The greatest acknowledged watermark whose live state equals `recovered`,
/// if any — the adoption point for a reopen that made no recovery claim.
fn matching_watermark(
    model: &ExpectedState,
    branch: BranchId,
    recovered: &super::recovery_oracle::verify::RecoveredState,
) -> Option<CommitVersion> {
    let upper = model.max_version(branch)?;
    model
        .candidate_watermarks(branch, upper)
        .into_iter()
        .find(|&watermark| &model.live_state_at(branch, watermark) == recovered)
}

/// How a reopen attempt resolved.
enum Reopened {
    /// Opened `Healthy`, classified clean at its claim; the model is adopted
    /// to the survivor and the epoch continues on this runtime.
    Survivor(StorageRuntime<'static>),
    /// Refused loudly (open or first scan) — fail-closed, the case ends.
    RefusedLoud,
    /// Opened with declared loss (`Degraded`/`Failed` health) onto a valid
    /// prefix of acked history — legal; mutating admission is blocked with no
    /// acknowledge path by design, so the case ends (whole-DB semantics).
    DegradedPrefix,
}

/// Reopens the (possibly damaged) store and holds it to the oracle: refuse
/// loudly, or be truthful about acked history. A `Healthy` open's
/// `recovered_visible_version` is a truth claim — the model is truncated to
/// it and classified under `ZeroLoss` strictness (a state below the claim is
/// a health-vs-truth violation). A `Degraded`/`Failed` open has DECLARED
/// loss, so a lower watermark is legal — but fabrication, holes, and torn
/// batches still violate (`OnDiskDamage` classification; the smoke's first
/// firing was exactly this false positive: Degraded + empty prefix judged at
/// Healthy strictness). Truncation-adoption keeps re-issued versions from
/// colliding with shed acks (#2864 semantics).
fn reopen_and_verify(
    root: &Path,
    model: &mut ExpectedState,
    branch: BranchId,
    outcome: &mut DualMutationOutcome,
) -> Result<Reopened, TestkitError> {
    let opened = crate::testkit::reopen_retry::open_with_retry_on_unavailable(|| {
        StorageRuntime::open_durable_local_with_options(root.to_path_buf(), open_options())
    });
    let Ok(open_outcome) = opened else {
        // Typed refusal of a damaged store — fail-closed, safe.
        outcome.loud_refusals += 1;
        return Ok(Reopened::RefusedLoud);
    };
    let (runtime, summary) = open_outcome.into_parts();
    let scanned = scan_recovered(
        &runtime,
        branch,
        &oracle_space(),
        &oracle_prefix_key(),
        SCAN_LIMIT,
    );
    let Ok(recovered) = scanned else {
        // The only fallible work at this site is the runtime's own read
        // path: a loud read refusal over damage is fail-closed, safe.
        outcome.read_refusals += 1;
        return Ok(Reopened::RefusedLoud);
    };
    let healthy = matches!(summary.recovery_health(), RecoveryHealthSummary::Healthy);
    let verdict = match (healthy, summary.recovered_visible_version()) {
        (true, Some(bound)) => {
            // Healthy: the claim is a truth claim — everything above `bound`
            // was shed; everything at or below it must be intact.
            model.truncate_branch_above(branch, bound);
            classify_recovered(model, branch, &recovered, CrashFamily::ZeroLoss)
        }
        (true, None) => match matching_watermark(model, branch, &recovered) {
            Some(watermark) => {
                model.truncate_branch_above(branch, watermark);
                Ok(())
            }
            None => classify_recovered(model, branch, &recovered, CrashFamily::OnDiskDamage),
        },
        (false, bound) => {
            // Declared loss: a lower watermark is legal, silent wrongness
            // is not — fabricated rows, holes, and torn batches still fail.
            if let Some(bound) = bound {
                model.truncate_branch_above(branch, bound);
            }
            classify_recovered(model, branch, &recovered, CrashFamily::OnDiskDamage)
        }
    };
    verdict.map_err(|violation| {
        TestkitError::new(format!("dual-mutation oracle violation: {violation:?}"))
    })?;
    if healthy {
        outcome.reopens_classified += 1;
        Ok(Reopened::Survivor(runtime))
    } else {
        outcome.degraded_prefixes += 1;
        Ok(Reopened::DegradedPrefix)
    }
}

/// True when the error's `source()` chain bottoms out in a backend
/// `AlreadyExists` — the #3015 snapshot-id collision signature. Structural
/// (downcast + `kind()`), never display text.
fn is_already_exists(err: &(dyn std::error::Error + 'static)) -> bool {
    let mut current: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(inner) = current {
        if let Some(backend) = inner.downcast_ref::<BackendError>() {
            return backend.kind() == BackendErrorKind::AlreadyExists;
        }
        current = inner.source();
    }
    false
}

/// One committed mutation batch drawn from the cursor. Ops run only on
/// stores that opened `Healthy` and passed claim-strict classification, so
/// any op error here is LOUD — an "open said Healthy, operation refused"
/// tolerance would mask findings (the #2828 allowance lesson); declared-loss
/// stores never reach this (`DegradedPrefix` ends the case at the reopen).
/// The single exception is the OPEN bug #3015's `AlreadyExists` signature,
/// counted and case-ending until its fix deletes the allowance. Returns
/// `false` when the case must end.
fn drive_op(
    runtime: &mut StorageRuntime<'static>,
    model: &mut ExpectedState,
    branch: BranchId,
    op_bytes: [u8; 3],
    outcome: &mut DualMutationOutcome,
) -> Result<bool, TestkitError> {
    let [verb, key_select, value_select] = op_bytes;
    let mutations = match verb % 8 {
        0..=3 => vec![RecordedMutation::Put {
            space: oracle_space(),
            key: oracle_key(key_select % KEY_SPACE),
            value: StorageValue::new(vec![0xD5, verb, key_select, value_select]),
        }],
        4 => vec![
            RecordedMutation::Put {
                space: oracle_space(),
                key: oracle_key(key_select % KEY_SPACE),
                value: StorageValue::new(vec![0xD6, verb, key_select, value_select]),
            },
            RecordedMutation::Put {
                space: oracle_space(),
                key: oracle_key(key_select.wrapping_add(1) % KEY_SPACE),
                value: StorageValue::new(vec![0xD7, verb, key_select, value_select]),
            },
        ],
        5 => vec![RecordedMutation::Delete {
            space: oracle_space(),
            key: oracle_key(key_select % KEY_SPACE),
        }],
        _ => {
            let task = if verb % 8 == 6 {
                MaintenanceTask::Flush
            } else {
                MaintenanceTask::Checkpoint
            };
            let request = MaintenanceRequest::new(task, MaintenanceScope::Branch(branch));
            let driven = runtime
                .enqueue_maintenance(&request)
                .and_then(|_| runtime.drain_maintenance());
            match driven {
                Ok(_) => {
                    if verb % 8 == 6 {
                        outcome.flushes += 1;
                    } else {
                        outcome.checkpoints += 1;
                    }
                }
                Err(err) if is_already_exists(&err) => {
                    // The #3015 collision: manifest loss reopened Healthy and
                    // the reseeded snapshot id hit an orphaned object.
                    outcome.pin_3015_snapshot_collisions += 1;
                    return Ok(false);
                }
                Err(crate::api::StorageApiError::MaintenanceRejected { .. }) => {
                    // Benign scheduling race on the real threaded runtime: the
                    // background maintenance lane picked the enqueued task up
                    // before this manual drain reached it ("no longer
                    // startable"). The work runs either way — count and
                    // continue; rejection is not failure.
                    outcome.maintenance_races += 1;
                }
                Err(err) => {
                    return Err(TestkitError::new(format!(
                        "maintenance on a healthy store: {err:?}"
                    )));
                }
            }
            return Ok(true);
        }
    };
    let batch = CommitBatch::new(
        branch,
        mutations.iter().map(to_commit_mutation).collect(),
        CommitOptions::default(),
    )
    .map_err(|err| TestkitError::new(format!("build batch: {err:?}")))?;
    let summary = runtime
        .commit(&batch)
        .map_err(|err| TestkitError::new(format!("commit on a healthy store: {err:?}")))?;
    model.record_ack(branch, summary.commit_version(), mutations);
    match verb % 8 {
        0..=3 => outcome.puts += 1,
        4 => outcome.paired_puts += 1,
        _ => outcome.deletes += 1,
    }
    Ok(true)
}

/// Runs one dual-mutation case: parse `data` as an epoch script over a real
/// durable store at `root`, interleaving operations with on-disk damage, and
/// judge every reopen with the recovery oracle. `Err` is a finding (or a
/// harness I/O failure) — the fuzz target panics on it.
pub fn check_dual_mutation_contract(
    root: &Path,
    data: &[u8],
) -> Result<DualMutationOutcome, TestkitError> {
    let mut outcome = DualMutationOutcome::default();
    let mut cursor = Cursor::new(data);
    let branch = default_branch();
    let mut model = ExpectedState::new(OracleDurability::Standard);
    let mut runtime = open_store(root)?.into_runtime();

    for _epoch in 0..MAX_EPOCHS {
        // Op phase.
        let Some(op_count) = cursor.next() else {
            break;
        };
        for _ in 0..op_count % (MAX_OPS_PER_EPOCH + 1) {
            let (Some(verb), Some(key_select), Some(value_select)) =
                (cursor.next(), cursor.next(), cursor.next())
            else {
                break;
            };
            if !drive_op(
                &mut runtime,
                &mut model,
                branch,
                [verb, key_select, value_select],
                &mut outcome,
            )? {
                // A known-issue allowance fired; quiesce and end the case.
                runtime
                    .close()
                    .map_err(|err| TestkitError::new(format!("allowance close: {err:?}")))?;
                return Ok(outcome);
            }
        }
        // Quiesce: a clean close releases workers and the writer lock, so
        // the damage below lands on stable, fsynced files.
        runtime
            .close()
            .map_err(|err| TestkitError::new(format!("clean close: {err:?}")))?;

        // Damage phase. (On input exhaustion the runtime is already closed —
        // return, not break, so the fall-through close isn't a double close.)
        let Some(damage_count) = cursor.next() else {
            outcome.epochs_completed += 1;
            return Ok(outcome);
        };
        for _ in 0..damage_count % (MAX_DAMAGE_PER_EPOCH + 1) {
            let (Some(file_select), Some(action_select), Some(offset_select)) =
                (cursor.next(), cursor.next(), cursor.next_u16())
            else {
                break;
            };
            apply_damage(
                root,
                file_select,
                action_select,
                offset_select,
                &mut outcome,
            )?;
        }

        // Reopen under the oracle; continue the next epoch on the survivor.
        match reopen_and_verify(root, &mut model, branch, &mut outcome)? {
            Reopened::Survivor(reopened) => {
                runtime = reopened;
                outcome.epochs_completed += 1;
            }
            Reopened::RefusedLoud | Reopened::DegradedPrefix => {
                outcome.epochs_completed += 1;
                return Ok(outcome);
            }
        }
    }
    // Hygiene: join workers at every case end. A dropped-open runtime only
    // detaches its background workers, and detached workers accumulating
    // across thousands of in-process fuzz execs starve the scheduler into
    // spurious transients.
    runtime
        .close()
        .map_err(|err| TestkitError::new(format!("case-end close: {err:?}")))?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed corpus seeds, exercised per-PR so they can never drift
    /// from the grammar (the format-fuzz `valid-*` grid pattern).
    const SEED_CLEAN_EPOCHS: &[u8] =
        include_bytes!("../../fuzz/corpus/dual_mutation/valid-clean-epochs");
    const SEED_DAMAGE_MIX: &[u8] =
        include_bytes!("../../fuzz/corpus/dual_mutation/valid-damage-mix");
    const SEED_DELETE_FILE: &[u8] =
        include_bytes!("../../fuzz/corpus/dual_mutation/valid-delete-file");
    /// The smoke's first firing (2026-09-01): WAL-erasing damage reopened
    /// `Degraded` with claim v2 over an empty prefix, and the pre-fix oracle
    /// judged the DECLARED loss at `Healthy` strictness — a harness false
    /// positive. Pinned so declared-loss reopens stay legal.
    const SEED_DEGRADED_LOSS: &[u8] =
        include_bytes!("../../fuzz/corpus/dual_mutation/valid-degraded-loss");
    /// The #3015 finding input (2026-09-01, the target's first product
    /// yield): manifest deletion → Healthy reopen → snapshot-id collision.
    const SEED_3015_COLLISION: &[u8] =
        include_bytes!("../../fuzz/corpus/dual_mutation/pin-3015-snapshot-collision");

    fn run(script: &[u8]) -> DualMutationOutcome {
        let dir = tempfile::tempdir().expect("tmp");
        check_dual_mutation_contract(dir.path(), script).expect("case runs clean")
    }

    #[test]
    fn a_clean_epoch_script_replays_identically_with_exact_counters() {
        let expected = DualMutationOutcome {
            epochs_completed: 3,
            puts: 4,
            paired_puts: 1,
            deletes: 1,
            flushes: 1,
            checkpoints: 1,
            reopens_classified: 3,
            ..DualMutationOutcome::default()
        };
        let first = run(SEED_CLEAN_EPOCHS);
        assert_eq!(first, expected, "every grammar arm lands exactly once-plus");
        assert_eq!(run(SEED_CLEAN_EPOCHS), first, "damage-free cases replay");
    }

    #[test]
    fn an_empty_input_is_a_clean_noop() {
        assert_eq!(run(&[]), DualMutationOutcome::default());
    }

    #[test]
    fn damage_directives_drive_their_arms() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        std::fs::create_dir(root.join("sub")).expect("subdir");
        std::fs::write(root.join("a.bin"), [7u8; 10]).expect("a");
        std::fs::write(root.join("sub/b.bin"), []).expect("b");
        let mut outcome = DualMutationOutcome::default();

        // Sorted listing: a.bin then sub/b.bin.
        let files = list_files(root).expect("list");
        assert_eq!(files, vec![root.join("a.bin"), root.join("sub/b.bin")]);

        // Flip byte 3 of a.bin.
        apply_damage(root, 0, 0, 3, &mut outcome).expect("flip");
        let mut expected = [7u8; 10];
        expected[3] ^= 0xFF;
        assert_eq!(std::fs::read(root.join("a.bin")).expect("read"), expected);

        // Flip on the empty file is skipped, not an error.
        apply_damage(root, 1, 0, 0, &mut outcome).expect("flip empty");

        // Append 1 + (8 % 64) junk bytes to a.bin.
        apply_damage(root, 0, 2, 8, &mut outcome).expect("append");
        let appended = std::fs::read(root.join("a.bin")).expect("read");
        assert_eq!(appended.len(), 19);
        assert!(appended[10..].iter().all(|&byte| byte == JUNK_BYTE));

        // Truncate a.bin to 5 bytes.
        apply_damage(root, 0, 1, 5, &mut outcome).expect("truncate");
        assert_eq!(
            std::fs::read(root.join("a.bin")).expect("read"),
            expected[..5]
        );

        // Delete sub/b.bin.
        apply_damage(root, 1, 3, 0, &mut outcome).expect("delete");
        assert!(!root.join("sub/b.bin").exists());

        // An empty tree skips instead of erroring.
        std::fs::remove_file(root.join("a.bin")).expect("clear");
        apply_damage(root, 0, 0, 0, &mut outcome).expect("empty tree");

        assert_eq!(
            outcome,
            DualMutationOutcome {
                damage_flips: 1,
                damage_appends: 1,
                damage_truncates: 1,
                damage_deletes: 1,
                damage_skipped: 2,
                ..DualMutationOutcome::default()
            }
        );
    }

    #[test]
    fn a_mixed_damage_script_survives_or_refuses_loudly() {
        let outcome = run(SEED_DAMAGE_MIX);
        // Epoch-0 ops land before any damage, so they are exact.
        assert_eq!(outcome.paired_puts, 1);
        assert_eq!(outcome.flushes, 1);
        assert!(outcome.puts == 1 || outcome.puts == 2, "{outcome:?}");
        // The three directives: append + truncate always land; the flip lands
        // unless the selected file is empty on this layout.
        assert_eq!(outcome.damage_appends, 1);
        assert_eq!(outcome.damage_truncates, 1);
        assert_eq!(outcome.damage_flips + outcome.damage_skipped, 1);
        // Every reopen resolves as exactly one of the oracle's safe outcomes.
        let reopen_facts = outcome.reopens_classified
            + outcome.degraded_prefixes
            + outcome.loud_refusals
            + outcome.read_refusals;
        assert!(reopen_facts == 1 || reopen_facts == 2, "{outcome:?}");
        assert!(outcome.epochs_completed >= 1, "{outcome:?}");
    }

    #[test]
    fn the_3015_finding_input_replays_clean_under_the_allowance() {
        // Grammar-level regression twin of the pin test: the exact fuzz input
        // that found #3015 must complete clean while the allowance stands.
        // (Which safe outcome it lands on is layout-dependent; the pin test
        // owns the sharp assertions.)
        let outcome = run(SEED_3015_COLLISION);
        let safe_ends = outcome.pin_3015_snapshot_collisions
            + outcome.reopens_classified
            + outcome.degraded_prefixes
            + outcome.loud_refusals
            + outcome.read_refusals;
        assert!(safe_ends >= 1, "{outcome:?}");
    }

    #[test]
    fn a_declared_loss_reopen_is_legal_not_a_violation() {
        // The regression pin for the smoke's false positive: this input must
        // complete clean, resolving its damaged reopen as one of the safe
        // outcomes (on the current layout, a degraded prefix).
        let outcome = run(SEED_DEGRADED_LOSS);
        assert_eq!(outcome.puts, 2, "{outcome:?}");
        assert_eq!(outcome.flushes, 2, "{outcome:?}");
        assert_eq!(
            outcome.degraded_prefixes + outcome.loud_refusals + outcome.read_refusals,
            1,
            "the damaged reopen resolves safely: {outcome:?}"
        );
        assert_eq!(outcome.epochs_completed, 2, "{outcome:?}");
    }

    #[test]
    fn erased_wal_segments_reopen_as_a_degraded_prefix() {
        // Deterministic construction of the declared-loss shape: unflushed
        // acks whose WAL segments are erased. Recovery must declare the loss
        // (Degraded) and present a valid earlier prefix — never serve the
        // claim as Healthy.
        let dir = tempfile::tempdir().expect("tmp");
        let (mutations, version) = store_with_one_put(dir.path(), b"real");
        let wal_dir = dir.path().join("wal");
        let mut erased = 0usize;
        for entry in std::fs::read_dir(&wal_dir).expect("wal dir") {
            let path = entry.expect("entry").path();
            if path.is_file() {
                std::fs::remove_file(&path).expect("erase segment");
                erased += 1;
            }
        }
        assert!(erased > 0, "the unflushed put must live in a WAL segment");
        let branch = default_branch();
        let mut model = ExpectedState::new(OracleDurability::Standard);
        model.record_ack(branch, version, mutations);
        let mut outcome = DualMutationOutcome::default();
        match reopen_and_verify(dir.path(), &mut model, branch, &mut outcome).expect("verify") {
            Reopened::DegradedPrefix | Reopened::RefusedLoud => {}
            Reopened::Survivor(_) => {
                panic!("erased acked history must not reopen as Healthy: {outcome:?}")
            }
        }
        assert_eq!(
            outcome.degraded_prefixes + outcome.loud_refusals + outcome.read_refusals,
            1,
            "{outcome:?}"
        );
    }

    #[test]
    fn pin_3015_manifest_loss_reopens_healthy_and_collides_on_checkpoint() {
        // Gate-7 pin of OPEN bug #3015, asserting the CURRENT broken
        // behavior: after manifest/current is deleted, the store reopens
        // HEALTHY (WAL replay restores the data, so classification passes),
        // and the next checkpoint collides with the orphaned snapshot object
        // (`AlreadyExists`), which the harness allows and counts. When the
        // fix lands this test flips — promote it to the fixed contract and
        // DELETE the drive_op allowance.
        let dir = tempfile::tempdir().expect("tmp");
        let branch = default_branch();
        let mut model = ExpectedState::new(OracleDurability::Standard);
        let mut outcome = DualMutationOutcome::default();
        let mut runtime = open_store(dir.path()).expect("open").into_runtime();
        assert!(
            drive_op(&mut runtime, &mut model, branch, [0, 0, 1], &mut outcome).expect("put"),
            "put continues"
        );
        assert!(
            drive_op(&mut runtime, &mut model, branch, [7, 0, 0], &mut outcome)
                .expect("checkpoint"),
            "checkpoint continues"
        );
        runtime.close().expect("close");
        std::fs::remove_file(dir.path().join("manifest/current.object@")).expect("delete manifest");

        let mut reopened = match reopen_and_verify(dir.path(), &mut model, branch, &mut outcome)
            .expect("verify")
        {
            Reopened::Survivor(runtime) => runtime,
            Reopened::RefusedLoud | Reopened::DegradedPrefix => panic!(
                "#3015 first half changed — manifest loss no longer reopens Healthy; \
                     promote this pin and delete the drive_op allowance"
            ),
        };
        let continued = drive_op(&mut reopened, &mut model, branch, [7, 0, 0], &mut outcome)
            .expect("the collision is allowed, not loud");
        assert!(
            !continued,
            "#3015 second half changed — checkpoint after manifest loss succeeded; \
             promote this pin and delete the drive_op allowance"
        );
        assert_eq!(outcome.pin_3015_snapshot_collisions, 1, "{outcome:?}");
        reopened.close().expect("close survivor");
    }

    #[test]
    fn a_corrupted_manifest_refuses_loudly() {
        // Deterministic fail-closed shape: a flipped byte in every manifest
        // object must refuse the open with a typed error — never open onto
        // unverifiable structure. (Probed 2026-09-01: a corrupted table
        // object ALSO refuses at open — recovery validates structural
        // objects up front, which is why `read_refusals` is a fail-safe.)
        let dir = tempfile::tempdir().expect("tmp");
        let (mutations, version) = store_with_one_put(dir.path(), b"real");
        let mut flipped = 0usize;
        for file in list_files(dir.path()).expect("list") {
            if file.to_string_lossy().contains("manifest") {
                let mut bytes = std::fs::read(&file).expect("read");
                if !bytes.is_empty() {
                    let mid = bytes.len() / 2;
                    bytes[mid] ^= 0xFF;
                    std::fs::write(&file, bytes).expect("write");
                    flipped += 1;
                }
            }
        }
        assert!(flipped > 0, "the store must have manifest objects");
        let branch = default_branch();
        let mut model = ExpectedState::new(OracleDurability::Standard);
        model.record_ack(branch, version, mutations);
        let mut outcome = DualMutationOutcome::default();
        match reopen_and_verify(dir.path(), &mut model, branch, &mut outcome).expect("verify") {
            Reopened::RefusedLoud => {}
            Reopened::Survivor(_) | Reopened::DegradedPrefix => {
                panic!("a corrupted manifest must refuse the open: {outcome:?}")
            }
        }
        assert_eq!(outcome.loud_refusals, 1, "{outcome:?}");
        assert_eq!(outcome.reopens_classified + outcome.degraded_prefixes, 0);
    }

    #[test]
    fn a_deleted_store_file_refuses_or_survives() {
        let outcome = run(SEED_DELETE_FILE);
        assert_eq!(outcome.puts, 1);
        assert_eq!(outcome.checkpoints, 1);
        assert_eq!(outcome.damage_deletes, 1);
        assert_eq!(
            outcome.reopens_classified
                + outcome.degraded_prefixes
                + outcome.loud_refusals
                + outcome.read_refusals,
            1,
            "{outcome:?}"
        );
        assert_eq!(outcome.epochs_completed, 1);
    }

    /// One committed put, closed clean; returns its version so tests can
    /// build truthful and poisoned models against the same on-disk store.
    fn store_with_one_put(root: &Path, value: &[u8]) -> (Vec<RecordedMutation>, CommitVersion) {
        let mut runtime = open_store(root).expect("open").into_runtime();
        let mutations = vec![RecordedMutation::Put {
            space: oracle_space(),
            key: oracle_key(0),
            value: StorageValue::new(value.to_vec()),
        }];
        let batch = CommitBatch::new(
            default_branch(),
            mutations.iter().map(to_commit_mutation).collect(),
            CommitOptions::default(),
        )
        .expect("batch");
        let version = runtime.commit(&batch).expect("commit").commit_version();
        runtime.close().expect("close");
        (mutations, version)
    }

    #[test]
    fn a_truthful_model_reopens_as_survivor() {
        let dir = tempfile::tempdir().expect("tmp");
        let (mutations, version) = store_with_one_put(dir.path(), b"real");
        let branch = default_branch();
        let mut model = ExpectedState::new(OracleDurability::Standard);
        model.record_ack(branch, version, mutations);
        let mut outcome = DualMutationOutcome::default();
        match reopen_and_verify(dir.path(), &mut model, branch, &mut outcome).expect("verify") {
            Reopened::Survivor(mut runtime) => {
                runtime.close().expect("close");
            }
            Reopened::RefusedLoud | Reopened::DegradedPrefix => {
                panic!("an undamaged store must reopen Healthy")
            }
        }
        assert_eq!(outcome.reopens_classified, 1);
    }

    #[test]
    fn a_wrong_recorded_value_is_a_violation() {
        let dir = tempfile::tempdir().expect("tmp");
        let (_, version) = store_with_one_put(dir.path(), b"real");
        let branch = default_branch();
        let mut model = ExpectedState::new(OracleDurability::Standard);
        model.record_ack(
            branch,
            version,
            vec![RecordedMutation::Put {
                space: oracle_space(),
                key: oracle_key(0),
                value: StorageValue::new(b"forged".to_vec()),
            }],
        );
        let mut outcome = DualMutationOutcome::default();
        let Err(err) = reopen_and_verify(dir.path(), &mut model, branch, &mut outcome) else {
            panic!("a store that diverges from acked history must red the oracle");
        };
        assert!(err.to_string().contains("violation"), "{err}");
    }

    #[test]
    fn a_torn_batch_in_the_model_is_reported() {
        let dir = tempfile::tempdir().expect("tmp");
        let (mut mutations, version) = store_with_one_put(dir.path(), b"real");
        // The model claims the acked batch carried a second mutation the
        // store never applied — recovered state is a torn version of it.
        mutations.push(RecordedMutation::Put {
            space: oracle_space(),
            key: oracle_key(1),
            value: StorageValue::new(b"never-applied".to_vec()),
        });
        let branch = default_branch();
        let mut model = ExpectedState::new(OracleDurability::Standard);
        model.record_ack(branch, version, mutations);
        let mut outcome = DualMutationOutcome::default();
        let Err(err) = reopen_and_verify(dir.path(), &mut model, branch, &mut outcome) else {
            panic!("a partially-applied acked batch must red the oracle");
        };
        assert!(err.to_string().contains("violation"), "{err}");
    }
}
