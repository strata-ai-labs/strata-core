//! TCP4.3b — exhaustive schedule exploration of the off-lock read protocol
//! (BS2.4 Model 2) on the REAL structures, targeting the #2682 torn-read
//! question. Only compiled with `RUSTFLAGS="--cfg loom"`.
//!
//! Unlike the 4.3a protocol-shaped model, these models drive the actual
//! product code end to end below `load_published_snapshot`:
//! [`BranchLocalState::append_committed_rows_atomically`] (the single
//! apply funnel, including commit-triggered rotation),
//! [`BranchLocalState::capture_snapshot`] + [`BranchSnapshotPublisher`] (the
//! real publish path through the seam-swapped [`BranchReadSlot`] cell),
//! [`load_from_registry`] (the real off-lock snapshot load), and
//! [`BranchReadView::scan_prefix`] (the real merge/filter read) — with the
//! memtable's real `RwLock` and the slot's real swap cell participating in
//! loom's exploration via the `crate::sync` seam.
//!
//! What the writer replicates is the documented commit discipline
//! (`execute_durable_commit`, bootstrap.rs): under one runtime-lock hold
//! (modeled by a loom mutex, as in production), apply the batch, republish
//! the snapshot if the apply rotated the active memtable, and only then
//! advance the visible frontier's atomic mirror (Release). The reader is
//! the production order: frontier (Acquire) FIRST, then the snapshot, then
//! the bounded scan.
//!
//! The oracle is #2682's invariant: a reader bounded at `V` sees every
//! batch with version ≤ `V` completely — each key's returned version
//! equals the newest batch ≤ `V` — or, below the first batch, nothing.
//! Two sabotage twins keep the oracle honest: publishing the frontier
//! before the apply must surface the torn/stale read, and rotating without
//! republishing must strand the next batch invisibly.
//!
//! TCP4.3c adds the maintenance-driven structural swap — the last #2682
//! surface: a concurrent FLUSH retires a frozen table for its built L0
//! twin through the real install
//! ([`BranchLocalState::replace_frozen_with_level_zero_tables_by_identity`],
//! the BS5.3b identity path) with the production choreography (clone the
//! frozen handle under a brief lock, build the twin off-lock, install +
//! republish under the lock). Readers must stay covered on BOTH sides of
//! the swap: old views keep the retired frozen table alive by `Arc`, new
//! views serve the twin. Its sabotage twin republishes a stale capture
//! over a newer one — the publish-regression bug shape — which the oracle
//! must catch.

use super::config::BranchRuntimeConfig;
use super::facts::{BranchLevel, BranchTableDescriptor};
use super::read::{BranchOwnedTable, BranchReadBound, BranchScanBounds};
use super::snapshot::{load_from_registry, BranchSnapshotPublisher, BranchSnapshotRegistry};
use super::state::BranchLocalState;
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    sort_table_rows_by_key, FrozenTable, ImmutableTableBuilder, ImmutableTableReader,
    TableBuilderConfig, TableIdentity, TableReaderConfig, TableRow, TableSummaryExtras,
};
use loom::sync::atomic::{AtomicU64, Ordering};
use loom::sync::{Arc as LoomArc, Mutex as LoomMutex};
use std::sync::Arc;
use strata_core::{BranchId, CommitVersion, Timestamp};

const KEYS: [u8; 2] = [b'a', b'b'];

fn branch() -> BranchId {
    BranchId::from_bytes([7; 16])
}

fn key(suffix: u8) -> PhysicalKey {
    PhysicalKey::new(
        branch(),
        "vis-loom",
        StorageSpaceId::engine(0x37).expect("engine storage space"),
        vec![b'k', suffix],
    )
    .expect("physical key")
}

fn prefix() -> PhysicalKey {
    PhysicalKey::new(
        branch(),
        "vis-loom",
        StorageSpaceId::engine(0x37).expect("engine storage space"),
        vec![b'k'],
    )
    .expect("prefix key")
}

/// One commit's rows: every key overwritten at `version`.
fn batch(version: u64) -> Vec<StorageRow> {
    KEYS.iter()
        .map(|suffix| {
            StorageRow::put(
                key(*suffix),
                CommitVersion::new(version),
                Timestamp::from_micros(version),
                Timestamp::MAX,
                version.to_le_bytes().to_vec(),
            )
        })
        .collect()
}

/// The shared "runtime": branch state + publisher behind the runtime-lock
/// analog, plus the lock-free reader handles (registry + frontier mirror),
/// exactly the production topology.
struct World {
    runtime: LoomMutex<(BranchLocalState, BranchSnapshotPublisher)>,
    registry: Arc<BranchSnapshotRegistry>,
    visible: AtomicU64,
}

fn world(rotation_bytes: usize) -> LoomArc<World> {
    let mut state = BranchLocalState::new(
        branch(),
        BranchRuntimeConfig::default()
            .with_active_rotation_bytes(rotation_bytes)
            .expect("rotation config"),
    )
    .expect("branch state");
    // Seed batch v=1, published, visible: readers always find a snapshot.
    state
        .append_committed_rows_atomically(batch(1))
        .expect("seed batch applies");
    let mut publisher = BranchSnapshotPublisher::new();
    publisher.publish_view(
        branch(),
        Arc::new(state.capture_snapshot().expect("seed snapshot")),
    );
    let registry = publisher.registry_handle();
    LoomArc::new(World {
        runtime: LoomMutex::new((state, publisher)),
        registry,
        visible: AtomicU64::new(1),
    })
}

/// The production commit discipline for one batch: apply → republish iff
/// rotated → advance the visible mirror (Release), all under the
/// runtime-lock hold (`execute_durable_commit`'s shape).
fn commit_batch(world: &World, version: u64) {
    let mut guard = world.runtime.lock().expect("runtime lock");
    let (state, publisher) = &mut *guard;
    let frozen_before = state.frozen_table_count();
    state
        .append_committed_rows_atomically(batch(version))
        .expect("batch applies");
    if state.frozen_table_count() != frozen_before {
        publisher.publish_view(
            branch(),
            Arc::new(state.capture_snapshot().expect("rotation snapshot")),
        );
    }
    world.visible.store(version, Ordering::Release);
}

/// The production off-lock read: frontier (Acquire) FIRST, then the
/// snapshot, then the real bounded scan. Returns the bound and each key's
/// returned version.
fn frontier_bounded_scan(world: &World) -> (u64, Vec<u64>) {
    let visible = world.visible.load(Ordering::Acquire);
    let view = load_from_registry(&world.registry, branch()).expect("branch snapshot published");
    let rows = view
        .scan_prefix(
            &BranchScanBounds::prefix(&prefix()),
            BranchReadBound::at_version(CommitVersion::new(visible)),
        )
        .expect("bounded prefix scan");
    let versions = rows
        .iter()
        .map(|row| row.row().commit_version().as_u64())
        .collect();
    (visible, versions)
}

/// #2682's invariant at bound `V` with full-overwrite batches 1..=latest:
/// every key present exactly once, every returned version equal, and equal
/// to `min(V, latest)` — a mixed set is a torn batch, a uniformly-low set
/// is a stale read beyond the bound.
fn assert_batch_atomicity(bound: u64, versions: &[u64], latest_committed: u64) {
    assert_eq!(
        versions.len(),
        KEYS.len(),
        "bound {bound}: expected every key, saw versions {versions:?}"
    );
    let expected = bound.min(latest_committed);
    for version in versions {
        assert_eq!(
            *version, expected,
            "torn or stale read at bound {bound}: versions {versions:?}, \
             expected uniform {expected}"
        );
    }
}

/// No-rotation regime: the published snapshot's LIVE active handle must
/// expose each batch exactly when the frontier admits it, under every
/// interleaving of the reader with the two committing writers.
#[test]
fn loom_live_memtable_scan_never_tears_a_batch() {
    loom::model(|| {
        let world = world(usize::MAX / 2);
        let writer = {
            let world = LoomArc::clone(&world);
            loom::thread::spawn(move || {
                commit_batch(&world, 2);
                commit_batch(&world, 3);
            })
        };
        let reader = {
            let world = LoomArc::clone(&world);
            loom::thread::spawn(move || {
                let (bound, versions) = frontier_bounded_scan(&world);
                assert_batch_atomicity(bound, &versions, 3);
            })
        };
        writer.join().expect("writer completes");
        reader.join().expect("reader completes");
        // Post-quiesce: the final state is fully visible.
        let (bound, versions) = frontier_bounded_scan(&world);
        assert_eq!(bound, 3);
        assert_batch_atomicity(bound, &versions, 3);
    });
}

/// Rotation regime (`rotation_bytes = 1`: every batch rotates): the
/// commit-triggered rotation republish must keep every reader covered —
/// the exact structural window where a missed republish strands rows in an
/// active memtable the published snapshot no longer references.
#[test]
fn loom_rotation_republish_keeps_readers_covered() {
    loom::model(|| {
        let world = world(1);
        let writer = {
            let world = LoomArc::clone(&world);
            loom::thread::spawn(move || {
                commit_batch(&world, 2);
                commit_batch(&world, 3);
            })
        };
        let reader = {
            let world = LoomArc::clone(&world);
            loom::thread::spawn(move || {
                let (bound, versions) = frontier_bounded_scan(&world);
                assert_batch_atomicity(bound, &versions, 3);
            })
        };
        writer.join().expect("writer completes");
        reader.join().expect("reader completes");
        let (bound, versions) = frontier_bounded_scan(&world);
        assert_eq!(bound, 3);
        assert_batch_atomicity(bound, &versions, 3);
    });
}

/// The `applied_not_visible` direction, exhaustively: a batch applied
/// without advancing the frontier stays hidden from every schedule's read.
#[test]
fn loom_applied_above_frontier_stays_hidden() {
    loom::model(|| {
        let world = world(usize::MAX / 2);
        let writer = {
            let world = LoomArc::clone(&world);
            loom::thread::spawn(move || {
                let mut guard = world.runtime.lock().expect("runtime lock");
                let (state, _publisher) = &mut *guard;
                // Applied above the frontier: no republish, no mirror store.
                state
                    .append_committed_rows_atomically(batch(2))
                    .expect("batch applies");
            })
        };
        let reader = {
            let world = LoomArc::clone(&world);
            loom::thread::spawn(move || {
                let (bound, versions) = frontier_bounded_scan(&world);
                assert_eq!(bound, 1, "frontier never advanced");
                assert_batch_atomicity(bound, &versions, 1);
            })
        };
        writer.join().expect("writer completes");
        reader.join().expect("reader completes");
    });
}

/// Sabotage twin 1 (non-vacuity): advancing the frontier BEFORE the apply
/// must let loom surface the torn/stale read — the per-row memtable write
/// locks make a mid-batch reader observable the moment the bound admits
/// the version early.
#[test]
#[should_panic(expected = "torn or stale read")]
fn loom_frontier_published_before_apply_is_caught() {
    loom::model(|| {
        let world = world(usize::MAX / 2);
        let writer = {
            let world = LoomArc::clone(&world);
            loom::thread::spawn(move || {
                // The bug under test: visibility first, apply second.
                world.visible.store(2, Ordering::Release);
                let mut guard = world.runtime.lock().expect("runtime lock");
                let (state, _publisher) = &mut *guard;
                state
                    .append_committed_rows_atomically(batch(2))
                    .expect("batch applies");
            })
        };
        let reader = {
            let world = LoomArc::clone(&world);
            loom::thread::spawn(move || {
                let (bound, versions) = frontier_bounded_scan(&world);
                assert_batch_atomicity(bound, &versions, 2);
            })
        };
        writer.join().expect("writer completes");
        reader.join().expect("reader completes");
    });
}

/// Sabotage twin 2 (the #2682 structural window): rotating WITHOUT the
/// republish leaves the published snapshot's active handle pointing at the
/// rotated-out table — the NEXT batch lands in an active the snapshot
/// cannot see, and a bounded reader goes stale beyond its frontier.
#[test]
#[should_panic(expected = "torn or stale read")]
fn loom_rotation_without_republish_is_caught() {
    loom::model(|| {
        let world = world(1);
        {
            // Batch 2 rotates (threshold 1) — deliberately skip the
            // republish, then advance the frontier anyway.
            let mut guard = world.runtime.lock().expect("runtime lock");
            let (state, _publisher) = &mut *guard;
            state
                .append_committed_rows_atomically(batch(2))
                .expect("batch applies");
            drop(guard);
            world.visible.store(2, Ordering::Release);
            // Batch 3 lands in the fresh active the stale snapshot lacks.
            let mut guard = world.runtime.lock().expect("runtime lock");
            let (state, _publisher) = &mut *guard;
            state
                .append_committed_rows_atomically(batch(3))
                .expect("batch applies");
            drop(guard);
            world.visible.store(3, Ordering::Release);
        }
        let (bound, versions) = frontier_bounded_scan(&world);
        assert_batch_atomicity(bound, &versions, 3);
    });
}

/// Build the flush's L0 twin of a frozen table from its rows — the
/// in-memory equivalent of the prepare phase's durable table build (the
/// unit-test `immutable_reader` recipe).
fn l0_twin(identity: &str, frozen: &FrozenTable) -> BranchOwnedTable {
    let mut rows: Vec<TableRow> = frozen
        .iter()
        .map(|row| TableRow::new(row.row().clone()))
        .collect();
    sort_table_rows_by_key(&mut rows);
    let identity = TableIdentity::new(identity).expect("table identity");
    let builder = ImmutableTableBuilder::new(TableBuilderConfig::default()).expect("builder");
    let artifact = builder
        .build_from_rows(identity.clone(), &rows)
        .expect("twin builds");
    let reader = ImmutableTableReader::open_bytes(
        identity,
        artifact.into_bytes(),
        TableReaderConfig::default(),
    )
    .expect("twin reader");
    let descriptor = BranchTableDescriptor::new(
        reader.facts().identity().clone(),
        reader.facts().clone(),
        BranchLevel::ZERO,
    )
    .expect("twin descriptor");
    let extras = TableSummaryExtras::from_rows(reader.rows()).expect("twin extras");
    BranchOwnedTable::new(branch(), descriptor, reader, extras).expect("twin owned table")
}

/// TCP4.3c: the flush structural swap under concurrency — a frozen table
/// retired for its L0 twin (real identity-install + republish, production
/// choreography) while a writer commits and a reader scans. Readers must
/// stay covered on both sides of the swap, and the swap must preserve
/// content exactly.
#[test]
fn loom_flush_install_keeps_readers_covered() {
    let mut model = loom::model::Builder::new();
    model.preemption_bound = Some(2);
    model.check(|| {
        // Threshold 1: the seed batch (v1) rotated into frozen[0].
        let world = world(1);
        let writer = {
            let world = LoomArc::clone(&world);
            loom::thread::spawn(move || commit_batch(&world, 2))
        };
        let flusher = {
            let world = LoomArc::clone(&world);
            // The immutable-table build (block encode + eager decode) is
            // deeper than loom's default coroutine stack.
            loom::thread::Builder::new()
                .stack_size(4 << 20)
                .spawn(move || {
                    // Production shape: clone the frozen handle under a brief
                    // lock, build the twin OFF-lock, install + republish under
                    // the lock.
                    let (frozen, identity) = {
                        let guard = world.runtime.lock().expect("runtime lock");
                        let (state, _publisher) = &*guard;
                        let frozen = state.frozen()[0].clone();
                        let identity = frozen.memory_state_identity();
                        (frozen, identity)
                    };
                    let twin = l0_twin("flush-l0-twin", &frozen);
                    let mut guard = world.runtime.lock().expect("runtime lock");
                    let (state, publisher) = &mut *guard;
                    state
                        .replace_frozen_with_level_zero_tables_by_identity(identity, vec![twin])
                        .expect("flush install");
                    publisher.publish_view(
                        branch(),
                        Arc::new(state.capture_snapshot().expect("post-install snapshot")),
                    );
                })
                .expect("flusher spawns")
        };
        let reader = {
            let world = LoomArc::clone(&world);
            loom::thread::spawn(move || {
                let (bound, versions) = frontier_bounded_scan(&world);
                assert_batch_atomicity(bound, &versions, 2);
            })
        };
        writer.join().expect("writer completes");
        flusher.join().expect("flusher completes");
        reader.join().expect("reader completes");

        // Post-quiesce: content preserved through the swap, structure moved.
        let (bound, versions) = frontier_bounded_scan(&world);
        assert_eq!(bound, 2);
        assert_batch_atomicity(bound, &versions, 2);
        let guard = world.runtime.lock().expect("runtime lock");
        let (state, _publisher) = &*guard;
        assert_eq!(state.owned_table_count(), 1, "the twin is installed at L0");
        assert_eq!(
            state.frozen_table_count(),
            1,
            "only batch 2's rotated table remains frozen"
        );
    });
}

/// Sabotage twin (publish regression): republishing a STALE capture over a
/// newer one leaves readers bounded beyond the published coverage — the
/// oracle must catch it. (The stale view's live active handle still covers
/// batch 2 — it WAS the active when batch 2 applied — so the loss surfaces
/// at batch 3, which landed in an active the stale capture has no handle
/// to.)
#[test]
#[should_panic(expected = "torn or stale read")]
fn loom_republishing_a_stale_capture_is_caught() {
    loom::model(|| {
        let world = world(1);
        let stale = {
            let guard = world.runtime.lock().expect("runtime lock");
            let (state, _publisher) = &*guard;
            Arc::new(state.capture_snapshot().expect("stale capture"))
        };
        commit_batch(&world, 2);
        commit_batch(&world, 3);
        {
            let mut guard = world.runtime.lock().expect("runtime lock");
            let (_state, publisher) = &mut *guard;
            // The bug under test: an old capture clobbers the newer publish.
            publisher.publish_view(branch(), stale);
        }
        let (bound, versions) = frontier_bounded_scan(&world);
        assert_batch_atomicity(bound, &versions, 3);
    });
}
