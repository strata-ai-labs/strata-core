//! Property tests: KV temporal reads match a reference timeline oracle, and a
//! branch forked at a version equals the source read as-of that version.
//!
//! A random sequence of put/delete commits is applied to a small key set; each
//! commit's version, timestamp, and resulting value are recorded into a
//! per-key timeline. The engine's `get`, `get_at_version`, and `get_at` are
//! then checked against that oracle across the whole observed range plus
//! boundary points (version 0 and MAX, timestamp EPOCH and MAX).

mod common;

use std::collections::BTreeSet;

use common::{branch, key, open_cache_database, space, value};
use proptest::prelude::*;
use strata_core::{CommitVersion, Timestamp};

const KEYS: [&[u8]; 3] = [b"ka", b"kb", b"kc"];

#[derive(Clone, Copy, Debug)]
enum KvOp {
    Put(usize, u8),
    Delete(usize),
}

fn any_kv_op() -> impl Strategy<Value = KvOp> {
    let keys = 0usize..KEYS.len();
    prop_oneof![
        (keys.clone(), any::<u8>()).prop_map(|(index, byte)| KvOp::Put(index, byte)),
        keys.prop_map(KvOp::Delete),
    ]
}

/// One committed change to a key; `value` is `None` for a tombstone.
#[derive(Clone)]
struct Event {
    version: CommitVersion,
    timestamp: Timestamp,
    value: Option<Vec<u8>>,
}

/// Value of the latest event at or before `version` (events are in version
/// order, so the last one within bound wins).
fn oracle_at_version(events: &[Event], version: CommitVersion) -> Option<Vec<u8>> {
    events
        .iter()
        .rfind(|event| event.version <= version)
        .and_then(|event| event.value.clone())
}

/// Value of the highest-version event at or before `timestamp` (MVCC as-of).
fn oracle_at_timestamp(events: &[Event], timestamp: Timestamp) -> Option<Vec<u8>> {
    events
        .iter()
        .filter(|event| event.timestamp <= timestamp)
        .max_by_key(|event| event.version)
        .and_then(|event| event.value.clone())
}

fn oracle_latest(events: &[Event]) -> Option<Vec<u8>> {
    events.last().and_then(|event| event.value.clone())
}

proptest! {
    #[test]
    fn kv_temporal_reads_match_timeline_oracle(
        ops in prop::collection::vec(any_kv_op(), 1..30),
    ) {
        let mut db = open_cache_database().expect("cache database opens");
        let mut kv = db.kv(branch("default"), space("default")).expect("kv service");

        let mut timelines: Vec<Vec<Event>> = vec![Vec::new(); KEYS.len()];
        let mut max_version = 0u64;

        for op in ops {
            match op {
                KvOp::Put(index, byte) => {
                    let outcome = kv
                        .put(key(KEYS[index]), value(&[byte]))
                        .expect("put commits")
                        .commit();
                    timelines[index].push(Event {
                        version: outcome.version(),
                        timestamp: outcome.timestamp(),
                        value: Some(vec![byte]),
                    });
                    max_version = max_version.max(outcome.version().as_u64());
                }
                KvOp::Delete(index) => {
                    let outcome = kv.delete(key(KEYS[index])).expect("delete");
                    // A delete only commits (and tombstones) when the key exists.
                    if let Some(commit) = outcome.commit() {
                        timelines[index].push(Event {
                            version: commit.version(),
                            timestamp: commit.timestamp(),
                            value: None,
                        });
                        max_version = max_version.max(commit.version().as_u64());
                    }
                }
            }
        }

        // Real commit versions and timestamps are the in-range points (commit
        // versions are not contiguous — control-plane commits interleave — so
        // only recorded commits are guaranteed to resolve to a retained
        // frontier). EPOCH/MAX and version 0/MAX are the out-of-range boundaries
        // checked separately below.
        let mut commit_versions: BTreeSet<CommitVersion> = BTreeSet::new();
        let mut commit_timestamps: BTreeSet<Timestamp> = BTreeSet::new();
        for timeline in &timelines {
            for event in timeline {
                commit_versions.insert(event.version);
                commit_timestamps.insert(event.timestamp);
            }
        }

        for (index, timeline) in timelines.iter().enumerate() {
            let target = key(KEYS[index]);

            let latest = kv.get(&target).expect("get").map(|read| read.as_bytes().to_vec());
            prop_assert_eq!(latest, oracle_latest(timeline));

            // Recorded commit versions resolve to a valid retained frontier; the
            // key may or may not have a value there (Ok/None per the oracle).
            for &version in &commit_versions {
                let actual = kv
                    .get_at_version(&target, version)
                    .expect("in-range version read succeeds")
                    .map(|read| read.as_bytes().to_vec());
                prop_assert_eq!(actual, oracle_at_version(timeline, version));
            }

            // Real commit timestamps resolve to a valid retained frontier.
            for &timestamp in &commit_timestamps {
                let actual = kv
                    .get_at(&target, timestamp)
                    .expect("in-range timestamp read succeeds")
                    .map(|read| read.as_bytes().to_vec());
                prop_assert_eq!(actual, oracle_at_timestamp(timeline, timestamp));
            }

            // Out-of-range reads are diagnostics, never clamp-to-latest or
            // ordinary absence (F7/F8): before the retained floor (version 0 /
            // EPOCH) and after the latest retained commit (version MAX /
            // Timestamp::MAX). Only meaningful once at least one commit exists.
            if max_version >= 1 {
                const HISTORY_CODE: &str = "history_unavailable.engine.persistence_history";
                let below_floor = kv
                    .get_at_version(&target, CommitVersion::new(0))
                    .expect_err("below-floor version is a diagnostic");
                prop_assert_eq!(below_floor.code(), HISTORY_CODE);
                let past_latest_version = kv
                    .get_at_version(&target, CommitVersion::MAX)
                    .expect_err("past-latest version is a diagnostic");
                prop_assert_eq!(past_latest_version.code(), HISTORY_CODE);
                let before_history = kv
                    .get_at(&target, Timestamp::EPOCH)
                    .expect_err("before-history timestamp is a diagnostic");
                prop_assert_eq!(before_history.code(), HISTORY_CODE);
                let after_latest = kv
                    .get_at(&target, Timestamp::MAX)
                    .expect_err("after-latest timestamp is a diagnostic");
                prop_assert_eq!(after_latest.code(), HISTORY_CODE);
            }
        }
    }

    #[test]
    fn fork_at_version_equals_source_as_of(
        bytes in prop::collection::vec(any::<u8>(), 1..15),
        fork_point in 0usize..15,
    ) {
        let mut db = open_cache_database().expect("cache database opens");
        let mut versions: Vec<CommitVersion> = Vec::new();
        {
            let mut kv = db.kv(branch("default"), space("default")).expect("kv service");
            for &byte in &bytes {
                let outcome = kv
                    .put(key(b"target"), value(&[byte]))
                    .expect("put commits")
                    .commit();
                versions.push(outcome.version());
            }
        }

        let fork_version = versions[fork_point % versions.len()];
        db.branches()
            .expect("branch service")
            .fork_at_version(&branch("default"), branch("snapshot"), fork_version)
            .expect("fork at version");

        let child_latest = db
            .kv(branch("snapshot"), space("default"))
            .expect("child kv")
            .get(&key(b"target"))
            .expect("child get")
            .map(|read| read.as_bytes().to_vec());
        let source_as_of = db
            .kv(branch("default"), space("default"))
            .expect("source kv")
            .get_at_version(&key(b"target"), fork_version)
            .expect("source as-of")
            .map(|read| read.as_bytes().to_vec());

        prop_assert_eq!(child_latest, source_as_of);
    }
}
