//! Shared deterministic workload + key conventions for the recovery oracle and
//! the fault-injection sweep, so both drive the same shape of durable writes and
//! the same verifier can enumerate the live state with one prefix scan.

use std::collections::BTreeSet;

use strata_core_next::BranchId;

use super::model::RecordedMutation;
use crate::api::{CommitMutation, StorageKey, StorageSpaceId, StorageValue};
use crate::testkit::rng::SplitMix64;

/// Shared non-empty key prefix for every workload key, so the verifier can
/// enumerate the full live state with one prefix scan (empty keys are rejected).
pub(crate) const ORACLE_PREFIX: u8 = 0x6f;
/// Small key space — the small-scope hypothesis keeps enumeration cheap.
pub(crate) const KEY_SPACE: u8 = 8;
/// Prefix-scan row cap; far above the small key space so a full scan is complete.
pub(crate) const SCAN_LIMIT: usize = 4096;

pub(crate) fn oracle_space() -> StorageSpaceId {
    StorageSpaceId::new(vec![0x20]).expect("oracle space")
}

pub(crate) fn oracle_prefix_key() -> StorageKey {
    StorageKey::new(vec![ORACLE_PREFIX]).expect("oracle prefix")
}

pub(crate) fn oracle_key(index: u8) -> StorageKey {
    StorageKey::new(vec![ORACLE_PREFIX, index]).expect("oracle key")
}

/// The runtime's default branch (`DEFAULT_BRANCH_ID`), created on open.
pub(crate) fn default_branch() -> BranchId {
    BranchId::from_bytes([0x01; BranchId::BYTE_LEN])
}

/// Deterministic workload: each op is a commit of 1..=3 unique-key put/delete
/// mutations over the small key space, a pure function of `(seed, op_count)`.
pub(crate) fn generate_workload(seed: u64, op_count: usize) -> Vec<Vec<RecordedMutation>> {
    let mut rng = SplitMix64::new(seed);
    let mut ops = Vec::with_capacity(op_count);
    for _ in 0..op_count {
        let mutation_count = 1 + usize::from(rng.gen_u8_below(3));
        let mut mutations = Vec::with_capacity(mutation_count);
        let mut used: BTreeSet<u8> = BTreeSet::new();
        for _ in 0..mutation_count {
            let mut index = rng.gen_u8_below(KEY_SPACE);
            while !used.insert(index) {
                index = (index + 1) % KEY_SPACE;
            }
            if rng.gen_bool() {
                let value = rng.gen_u8_below(255);
                mutations.push(RecordedMutation::Put {
                    space: oracle_space(),
                    key: oracle_key(index),
                    value: StorageValue::new(vec![value]),
                });
            } else {
                mutations.push(RecordedMutation::Delete {
                    space: oracle_space(),
                    key: oracle_key(index),
                });
            }
        }
        ops.push(mutations);
    }
    ops
}

pub(crate) fn to_commit_mutation(mutation: &RecordedMutation) -> CommitMutation {
    match mutation {
        RecordedMutation::Put { space, key, value } => CommitMutation::Put {
            storage_space: space.clone(),
            key: key.clone(),
            value: value.clone(),
            ttl: None,
        },
        RecordedMutation::Delete { space, key } => CommitMutation::Delete {
            storage_space: space.clone(),
            key: key.clone(),
        },
    }
}
