//! BS2.4b — deterministic, in-repo perf gates (control-first Tier 1).
//!
//! The ≥3×@10M read-throughput headline is out-of-band (a `benchmarks/` bin on a big machine — see
//! the concurrent-reads bin). These two tests are the CI-gateable proofs of the *mechanism*: reads
//! are structurally off-lock, and commits stayed O(1) (Model 2 — no per-commit snapshot capture).

use super::*;

/// Lock-elision guard: the four off-lock read verbs load their branch through
/// `load_published_snapshot` (visible atomic + one `ArcSwap` load — no `slot.lock()`). A regression
/// signal if a read verb is ever re-coupled to the runtime lock.
#[test]
fn read_verbs_route_through_the_off_lock_snapshot_load() {
    assert!(
        RUNTIME_SOURCE.contains("fn load_published_snapshot"),
        "the off-lock snapshot-load helper must exist"
    );
    let uses = RUNTIME_SOURCE
        .matches("self.load_published_snapshot(")
        .count();
    assert!(
        uses >= 4,
        "expected all four read verbs to load off-lock via load_published_snapshot, found {uses}"
    );
}

/// Commit stays O(1): Model 2 does not capture/republish the snapshot per commit, so
/// `read_view_captures` does not scale with the commit count. This is the counter that would have
/// caught the BS2.3 Model-1 regression (a per-commit `capture_read_view`).
#[cfg(feature = "perf-trace")]
#[test]
fn commit_does_not_scale_snapshot_captures() {
    use crate::observability::perf_trace;
    const COMMITS: u64 = 64;

    let runtime = StorageRuntime::open_ephemeral()
        .expect("open ephemeral runtime")
        .into_runtime();
    let branch = StorageRuntime::default_branch_id_for_test();
    let _perf_trace = perf_trace::begin_test_capture();

    for value in 0..COMMITS {
        let batch = CommitBatch::new(
            branch,
            vec![CommitMutation::Put {
                storage_space: background_space(),
                key: key(b"o1"),
                value: StorageValue::new(value.to_be_bytes().to_vec()),
                ttl: None,
            }],
            CommitOptions::default().require_conflict_check(false),
        )
        .expect("valid batch");
        runtime.commit(&batch).expect("commit");
    }

    let captures = perf_trace::snapshot().read_view_captures();
    assert!(
        captures < COMMITS / 4,
        "per-commit snapshot capture regressed (Model 1): {captures} captures for {COMMITS} commits"
    );
}
