//! #2521/#2522 regression suite: fork state and timeline coverage across
//! RESTARTS. Every prior fork test was in-process; both defects only
//! manifested through recovery (catalog restore rebuilt a forked branch's
//! retained-timeline index as complete-but-EMPTY, erasing inherited pre-fork
//! coverage; the engine then silently forked an "empty" source into an
//! unparented empty child). These tests drive fork → own write → close →
//! reopen — the CLI's one-process-per-command shape — and pin:
//! fork-of-a-fork inheritance (#2521), pre-fork as-of resolution on forks
//! and grandforks (#2522), and empty-source fork-at-zero semantics with
//! parent linkage intact. Gated on `localfs` (durable reopen is the point).

use super::*;

fn open_durable_runtime(root: std::path::PathBuf) -> StorageRuntime<'static> {
    StorageRuntime::open_local(root)
        .expect("open durable runtime")
        .into_runtime()
}

fn default_branch() -> BranchId {
    StorageRuntime::default_branch_id_for_test()
}

fn fork_branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn engine_space() -> StorageSpaceId {
    StorageSpaceId::new(vec![0x20]).expect("engine storage space")
}

fn api_key(bytes: &[u8]) -> StorageKey {
    StorageKey::new(bytes.to_vec()).expect("valid API key")
}

fn put(runtime: &mut StorageRuntime<'static>, branch_id: BranchId, value: &[u8], ts: u64) {
    let batch = CommitBatch::new(
        branch_id,
        vec![CommitMutation::Put {
            storage_space: engine_space(),
            key: api_key(b"city"),
            value: StorageValue::new(value.to_vec()),
            ttl: None,
        }],
        CommitOptions::default().require_conflict_check(false),
    )
    .expect("valid put batch");
    runtime
        .commit_for_test(&batch, Timestamp::from_micros(ts))
        .expect("commit put");
}

fn fork_current(runtime: &mut StorageRuntime<'static>, child: BranchId, source: BranchId) {
    runtime
        .branch(&BranchRequest::new(
            child,
            BranchAction::ForkCurrent { source },
            Some(BranchGeneration::new(1)),
        ))
        .expect("fork current");
}

fn read_at(
    runtime: &StorageRuntime<'static>,
    branch_id: BranchId,
    bound: ReadBound,
) -> Option<Vec<u8>> {
    runtime
        .read_point(&PointReadRequest::new(
            branch_id,
            engine_space(),
            api_key(b"city"),
            bound,
        ))
        .expect("point read")
        .row()
        .map(|row| row.value().expect("put row").as_bytes().to_vec())
}

/// #2521: the CLI repro shape, one reopen per step. The grandchild must read
/// the middle branch's post-fork write; the middle branch stays intact.
#[test]
fn fork_of_a_fork_inherits_the_middle_branch_across_reopens() {
    let root = temp_dir_for_api_test("fork-of-fork-reopen");
    let feature = fork_branch_id(0xA1);
    let grandchild = fork_branch_id(0xA2);

    {
        let mut runtime = open_durable_runtime(root.clone());
        put(&mut runtime, default_branch(), b"paris", 10);
        runtime.close().expect("close");
    }
    {
        let mut runtime = open_durable_runtime(root.clone());
        fork_current(&mut runtime, feature, default_branch());
        runtime.close().expect("close");
    }
    {
        let mut runtime = open_durable_runtime(root.clone());
        put(&mut runtime, feature, b"tokyo", 20);
        runtime.close().expect("close");
    }
    {
        let mut runtime = open_durable_runtime(root.clone());
        fork_current(&mut runtime, grandchild, feature);
        runtime.close().expect("close");
    }

    let runtime = open_durable_runtime(root);
    assert_eq!(
        read_at(&runtime, grandchild, ReadBound::Latest),
        Some(b"tokyo".to_vec()),
        "the grandchild must inherit the middle branch's post-fork write"
    );
    assert_eq!(
        read_at(&runtime, feature, ReadBound::Latest),
        Some(b"tokyo".to_vec()),
        "the middle branch stays intact"
    );
    assert_eq!(
        read_at(&runtime, default_branch(), ReadBound::Latest),
        Some(b"paris".to_vec()),
        "the root branch stays intact"
    );
}

/// #2522: a fork (and a grandfork) reopened from disk must resolve an as-of
/// read at a PRE-fork timestamp through the inherited timeline coverage.
#[test]
fn fork_resolves_pre_fork_as_of_across_reopens() {
    let root = temp_dir_for_api_test("fork-as-of-reopen");
    let fork = fork_branch_id(0xB1);
    let grand = fork_branch_id(0xB2);

    {
        let mut runtime = open_durable_runtime(root.clone());
        put(&mut runtime, default_branch(), b"paris", 10);
        put(&mut runtime, default_branch(), b"london", 20);
        runtime.close().expect("close");
    }
    {
        let mut runtime = open_durable_runtime(root.clone());
        fork_current(&mut runtime, fork, default_branch());
        runtime.close().expect("close");
    }
    {
        let mut runtime = open_durable_runtime(root.clone());
        put(&mut runtime, fork, b"tokyo", 30);
        runtime.close().expect("close");
    }
    {
        let mut runtime = open_durable_runtime(root.clone());
        fork_current(&mut runtime, grand, fork);
        runtime.close().expect("close");
    }

    let runtime = open_durable_runtime(root);
    let t1 = ReadBound::AtTimestamp(Timestamp::from_micros(10));
    assert_eq!(
        read_at(&runtime, default_branch(), t1),
        Some(b"paris".to_vec()),
        "parent as-of t1"
    );
    assert_eq!(
        read_at(&runtime, fork, t1),
        Some(b"paris".to_vec()),
        "fork as-of a pre-fork timestamp resolves through inherited coverage"
    );
    assert_eq!(
        read_at(&runtime, grand, t1),
        Some(b"paris".to_vec()),
        "grandfork as-of a pre-fork timestamp resolves through two hops"
    );
    assert_eq!(
        read_at(&runtime, grand, ReadBound::Latest),
        Some(b"tokyo".to_vec()),
        "grandfork head reads the middle branch's write"
    );
    // fork == as-of-V equivalence: the fork's own post-fork write is visible
    // at its own timestamp and invisible before it.
    assert_eq!(
        read_at(
            &runtime,
            fork,
            ReadBound::AtTimestamp(Timestamp::from_micros(30))
        ),
        Some(b"tokyo".to_vec()),
    );
    assert_eq!(
        read_at(
            &runtime,
            fork,
            ReadBound::AtTimestamp(Timestamp::from_micros(20))
        ),
        Some(b"london".to_vec()),
    );
}

/// #2521 (engine fallback removal): forking a branch with NO commit history
/// is the legitimate empty-fork case — storage forks at version zero and the
/// child keeps its parent linkage instead of degrading to an unparented
/// create.
#[test]
fn fork_of_an_empty_branch_keeps_parent_linkage() {
    let root = temp_dir_for_api_test("fork-empty-source");
    let empty_parent = fork_branch_id(0xC1);
    let child = fork_branch_id(0xC2);

    let mut runtime = open_durable_runtime(root);
    runtime
        .branch(&BranchRequest::new(
            empty_parent,
            BranchAction::Create,
            Some(BranchGeneration::new(1)),
        ))
        .expect("create empty parent");
    fork_current(&mut runtime, child, empty_parent);
    let described = runtime
        .branch(&BranchRequest::new(child, BranchAction::Describe, None))
        .expect("describe child");
    let summary = described.branches().first().expect("child summary");
    let parent = summary.parent().expect("child must keep parent linkage");
    assert_eq!(parent.source_branch_id(), empty_parent);
    assert_eq!(parent.fork_version(), CommitVersion::ZERO);
    assert_eq!(read_at(&runtime, child, ReadBound::Latest), None);
}
